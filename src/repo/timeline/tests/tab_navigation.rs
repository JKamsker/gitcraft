/// Undo three commits and commit something new: the three are a branch of the tree, a jump
/// brings back the newest of them (and undoing from there walks it), and a jump back to the new
/// commit, kept only by the timeline by then, works too.
#[test]
fn the_undo_tree_keeps_an_undone_future_to_jump_back_to() {
    let dir = repo("tree");
    let ctx = egui::Context::default();
    let base = git::head(&dir);
    let mut tab = open(&dir, &ctx);
    let c1 = commit(&mut tab, &ctx, "one.txt", "one");
    let c2 = commit(&mut tab, &ctx, "two.txt", "two");
    let c3 = commit(&mut tab, &ctx, "three.txt", "three");
    let third = tab.timeline.current;
    for _ in 0..3 {
        act(&mut tab, &ctx, Action::Undo);
    }
    let all_undone = git::head(&dir);
    let other = commit(&mut tab, &ctx, "other.txt", "other");
    let other_id = tab.timeline.current;
    let other_parent = tab.timeline.current_entry().and_then(|e| e.parent);
    let (_, lanes) = super::tree::layout(&tab.timeline);
    tab.toggle_timeline();
    act(&mut tab, &ctx, Action::Timeline(TimelineAct::Jump(third)));
    let after_jump = git::head(&dir);
    let at_third = tab.timeline.current;
    act(&mut tab, &ctx, Action::Undo);
    let after_undo = git::head(&dir);
    act(
        &mut tab,
        &ctx,
        Action::Timeline(TimelineAct::Jump(other_id)),
    );
    let back_to_other = git::head(&dir);
    let entries = tab.timeline.entries.len();
    let outside = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.origin == Origin::Outside)
        .count();
    let _ = std::fs::remove_dir_all(&dir);

    assert_ne!(c1, c2);
    assert_eq!(all_undone, base);
    assert_eq!(other_parent, None);
    assert_eq!(lanes, 2);
    assert_eq!(after_jump, c3);
    assert_eq!(at_third, third);
    assert_eq!(after_undo, c2);
    assert_eq!(back_to_other, other);
    assert_eq!(entries, 4);
    assert_eq!(outside, 0);
}

/// A jump checks the branches it moves are where they were read: one committed to since (not
/// read yet) stops it before anything runs.
#[test]
fn a_jump_refuses_once_a_branch_moved_since_it_was_read() {
    let dir = repo("jump-moved");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "a.txt", "a");
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "since"]);
    let since = git::head(&dir);
    tab.toggle_timeline();
    tab.apply(Action::Timeline(TimelineAct::Jump(None)), &ctx);
    let ran = wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let status = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    let head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ran);
    assert!(status.contains("has moved since"), "{status}");
    assert_eq!(head, since);
}

/// Forgetting the timeline lets go of what it kept.
#[test]
fn forgetting_the_timeline_lets_go_of_its_refs() {
    let dir = repo("forget");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "a.txt", "a");
    let id = tab.timeline.current.unwrap_or_default();
    let kept = wait(&mut tab, &ctx, |t| keeper_exists(&t.path, id));
    tab.toggle_timeline();
    tab.apply(Action::Timeline(TimelineAct::Forget(0)), &ctx);
    let released = wait(&mut tab, &ctx, |t| !keeper_exists(&t.path, id));
    let empty = tab.timeline.entries.is_empty();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(kept && released && empty);
}

/// An operation started before the read after the one before it came in: both are entries, in
/// order, and Ctrl+Z waits (saying so) until the timeline knows where the repository is.
#[test]
fn back_to_back_operations_are_both_kept() {
    let dir = repo("back-to-back");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let _ = std::fs::write(dir.join("a.txt"), "a");
    git_ok(&dir, &["add", "a.txt"]);
    tab.commit_summary = "first".to_owned();
    tab.apply(Action::Commit, &ctx);
    assert!(wait(&mut tab, &ctx, |t| t.busy_op.is_none()));
    // Its read may still be on the way.
    let refused = if tab.timeline.awaiting == super::Awaiting::Nothing {
        None
    } else {
        tab.undo(&ctx);
        Some(
            tab.status
                .as_deref()
                .cloned()
                .map(|(s, _)| s)
                .unwrap_or_default(),
        )
    };
    let _ = std::fs::write(dir.join("b.txt"), "b");
    // The cached status may still show the first commit's staged file. Give
    // this operation its input without waiting for that status refresh.
    git_ok(&dir, &["add", "b.txt"]);
    tab.commit_summary = "second".to_owned();
    tab.apply(Action::Commit, &ctx);
    let second_ran = tab.busy_op.is_some();
    assert!(wait(&mut tab, &ctx, settled));
    let status = tab.status.as_deref().cloned();
    let labels: Vec<String> = tab
        .timeline
        .entries
        .iter()
        .map(|e| e.label.clone())
        .collect();
    let named = wait(&mut tab, &ctx, |t| {
        t.timeline.entries.iter().all(|e| e.label.contains('“'))
    });
    let path = tab.timeline.path_to(tab.timeline.current);
    let subjects = git_ok(&dir, &["log", "--format=%s"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(second_ran, "{status:?}");
    assert_eq!(labels.len(), 2, "{labels:?} {status:?} {refused:?}");
    assert!(named);
    assert_eq!(path.len(), 2);
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["second", "first", "base"]
    );
    if let Some(status) = refused {
        assert!(status.contains("waiting for the repository"), "{status}");
    }
}

/// A commit in another worktree of the repository is that worktree's: it is no entry here, and
/// Ctrl+Z here still undoes this tab's own last operation.
#[test]
fn work_in_another_worktree_stays_off_this_timeline() {
    let dir = repo("worktrees");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let made = commit(&mut tab, &ctx, "a.txt", "mine");
    let wt = dir.with_file_name(format!(
        "{}-wt",
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&wt);
    git_ok(
        &dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            &wt.to_string_lossy(),
        ],
    );
    git_ok(&wt, &["commit", "-q", "--allow-empty", "-m", "theirs"]);
    tab.reload(&ctx);
    assert!(wait(&mut tab, &ctx, |t| settled(t) && t.worktrees.len() == 2));
    let entries = tab.timeline.entries.len();
    act(&mut tab, &ctx, Action::Undo);
    let head = git::head(&dir);
    let _ = git::run(
        Some(&dir),
        &["worktree", "remove", "--force", &wt.to_string_lossy()],
    );
    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(entries, 1);
    assert_ne!(head, made);
}
