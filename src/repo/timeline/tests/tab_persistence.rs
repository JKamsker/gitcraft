/// What gitcraft did is on disk once it is done: a tab opened again on the folder has it, and
/// undoes and redoes it.
#[test]
fn operations_are_kept_across_a_restart_and_undo_still_works() {
    let dir = repo("restart");
    let ctx = egui::Context::default();
    let base = git::head(&dir);
    let mut tab = open(&dir, &ctx);
    let made = commit(&mut tab, &ctx, "dir with space ü.txt", "Add the ü file");
    let id = tab.timeline.current_entry().map_or(0, |e| e.id);
    // Its files and kept commits come in the background, then it is written.
    let found = wait(&mut tab, &ctx, |t| {
        t.timeline.get(id).is_some_and(|e| !e.files.is_empty())
    });
    let kept = keeper_exists(&dir, id);
    let on_disk = wait(&mut tab, &ctx, |t| {
        store::load(&t.path).is_ok_and(|s| s.entries.first().is_some_and(|e| !e.files.is_empty()))
    });
    // Snapshot after enrichment: the worker may already finish during commit's
    // reload, so observing its temporary "Commit" label is not deterministic.
    let entry = tab.timeline.get(id).cloned();
    drop(tab);

    let mut again = open(&dir, &ctx);
    let restored = again.timeline.current_entry().cloned();
    act(&mut again, &ctx, Action::Undo);
    let undone = git::head(&dir);
    let outside = again
        .timeline
        .entries
        .iter()
        .any(|e| e.origin == Origin::Outside);
    act(&mut again, &ctx, Action::Redo);
    let redone = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    let entry = entry.unwrap_or_else(|| unreachable!("no entry"));
    assert_eq!(entry.label, "Commit “Add the ü file”");
    assert_eq!(entry.origin, Origin::Gitcraft);
    assert_eq!(entry.before.head, base);
    assert_eq!(entry.after.head, made);
    assert!(entry.undo.is_some());
    assert!(found && kept && on_disk);
    let restored = restored.unwrap_or_else(|| unreachable!("nothing restored"));
    assert_eq!(restored.id, id);
    // Named by its message once read, and kept so.
    assert_eq!(restored.label, "Commit “Add the ü file”");
    assert_eq!(restored.files, ["dir with space ü.txt"]);
    assert_eq!(undone, base);
    assert!(!outside, "undoing was taken for a change made elsewhere");
    assert_eq!(redone, made);
}

/// A commit, a branch and a reset made in the terminal show up, named from the reflog; one made
/// while the tab was closed too, once it opens; and undoing one puts the branches back.
#[test]
fn changes_made_elsewhere_show_up_named_from_the_reflog() {
    let dir = repo("outside");
    let ctx = egui::Context::default();
    let base = git::head(&dir);
    let mut tab = open(&dir, &ctx);
    let _ = std::fs::write(dir.join("p.txt"), "p\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "Fix the ü parser"]);
    tab.reload(&ctx);
    let labelled = |label: &'static str| {
        move |t: &RepoTab| t.timeline.current_entry().is_some_and(|e| e.label == label)
    };
    let committed = wait(&mut tab, &ctx, labelled("Commit “Fix the ü parser”"));
    git_ok(&dir, &["checkout", "-q", "-b", "fëature/ünï"]);
    tab.reload(&ctx);
    let checked_out = wait(&mut tab, &ctx, labelled("Checkout fëature/ünï"));
    let origins: Vec<(Origin, String)> = tab
        .timeline
        .entries
        .iter()
        .map(|e| (e.origin, e.label.clone()))
        .collect();
    drop(tab);

    // While closed.
    git_ok(&dir, &["checkout", "-q", "main"]);
    git_ok(&dir, &["reset", "-q", "--hard", "HEAD~1"]);
    let mut tab = open(&dir, &ctx);
    let while_closed = wait(&mut tab, &ctx, labelled("Reset to HEAD~1"));
    let last = tab.timeline.current_entry().cloned();
    // Undone: main goes back to the commit made in the terminal, the branch checked out too.
    act(&mut tab, &ctx, Action::Undo);
    let head_after_undo = git_ok(&dir, &["symbolic-ref", "--short", "HEAD"]);
    let main_after_undo = git_ok(&dir, &["rev-parse", "main"]);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(committed && checked_out, "{origins:?}");
    assert!(origins.iter().all(|(o, _)| *o == Origin::Outside));
    assert_eq!(origins.len(), 2);
    assert!(while_closed, "{last:?}");
    let last = last.unwrap_or_else(|| unreachable!());
    assert_eq!(last.after.head, base);
    assert_eq!(head_after_undo, "fëature/ünï");
    assert_eq!(
        main_after_undo,
        last.before
            .branches
            .get("main")
            .cloned()
            .unwrap_or_default()
    );
}

/// Yesterday's rebase, in a gitcraft opened today: its old tip is still there after the
/// reflogs expired and gc ran, and comes back as a branch, or puts the branch back.
#[test]
fn a_rebase_gives_its_old_tip_back_after_a_restart_and_a_gc() {
    let dir = repo("rebase");
    let ctx = egui::Context::default();
    git_ok(&dir, &["checkout", "-q", "-b", "feat"]);
    for n in ["f1", "f2"] {
        let _ = std::fs::write(dir.join(format!("{n}.txt")), n);
        git_ok(&dir, &["add", "-A"]);
        git_ok(&dir, &["commit", "-q", "-m", n]);
    }
    let old_tip = git::head(&dir);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let _ = std::fs::write(dir.join("m.txt"), "m");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "m1"]);
    git_ok(&dir, &["checkout", "-q", "feat"]);
    let mut tab = open(&dir, &ctx);
    let onto = git_ok(&dir, &["rev-parse", "refs/heads/main"]);
    tab.apply(rebase_action(BranchRef::local("main"), "feat"), &ctx);
    let accepted = tab
        .accept_rebase_for_test(&ctx, "feat", &old_tip, &onto)
        .unwrap_or_else(|error| unreachable!("rebase confirmation: {error}"));
    act(&mut tab, &ctx, accepted);
    let entry = tab.timeline.current_entry().cloned();
    let id = entry.as_ref().map_or(0, |e| e.id);
    let kept = wait(&mut tab, &ctx, |t| keeper_exists(&t.path, id));
    let saved = wait(&mut tab, &ctx, |t| {
        store::load(&t.path).is_ok_and(|s| s.entries.iter().any(|e| e.id == id))
    });
    drop(tab);
    // Nothing but the timeline keeps the old commits now.
    git_ok(
        &dir,
        &[
            "reflog",
            "expire",
            "--expire=now",
            "--expire-unreachable=now",
            "--all",
        ],
    );
    git_ok(&dir, &["gc", "-q", "--prune=now"]);
    let survived = git::run(
        Some(&dir),
        &["cat-file", "-e", &format!("{old_tip}^{{commit}}")],
    );
    // The graph doesn't show what is kept.
    let graph = git::log(&dir, None, git::LogOrder::default(), 0, 100).unwrap_or_default();

    let mut tab = open(&dir, &ctx);
    tab.toggle_timeline();
    tab.apply(Action::Timeline(TimelineAct::Select(Some(id))), &ctx);
    act(
        &mut tab,
        &ctx,
        Action::Timeline(TimelineAct::BranchAt {
            branch: "feat".to_owned(),
            commit: old_tip.clone(),
        }),
    );
    let name = format!("feat-was-{}", &old_tip[..8]);
    let branch = git::resolve(&dir, &format!("refs/heads/{name}"));
    act(
        &mut tab,
        &ctx,
        Action::Timeline(TimelineAct::PutBack {
            branch: "feat".to_owned(),
            commit: old_tip.clone(),
        }),
    );
    let feat = git::head(&dir);
    let put_back = tab.timeline.current_entry().map(|e| e.label.clone());
    let _ = std::fs::remove_dir_all(&dir);

    let entry = entry.unwrap_or_else(|| unreachable!());
    assert_eq!(entry.label, "Rebase feat onto main");
    assert_eq!(entry.before.branches.get("feat"), Some(&old_tip));
    assert!(kept && saved);
    assert!(survived.is_ok(), "{survived:?}");
    assert!(
        graph
            .iter()
            .all(|c| !c.summary.starts_with("gitcraft timeline"))
    );
    assert!(graph.iter().all(|c| c.hash != old_tip));
    assert_eq!(branch, Ok(old_tip.clone()));
    assert_eq!(feat, old_tip);
    assert_eq!(
        put_back.as_deref(),
        Some(&*format!("Put feat back at {}", &old_tip[..8]))
    );
}
