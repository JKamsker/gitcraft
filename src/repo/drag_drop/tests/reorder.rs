// Reorder regression coverage.

/// `main` with base, then a, b and c.
fn three(name: &str) -> (PathBuf, [String; 3]) {
    let dir = repo(name);
    let a = commit(&dir, "a.txt", "a");
    let b = commit(&dir, "b.txt", "b");
    let c = commit(&dir, "c.txt", "c");
    (dir, [a, b, c])
}

#[test]
fn a_commit_dropped_on_another_of_the_branch_moves_there_and_undo_puts_it_back() {
    let (dir, [a, _b, c]) = three("reorder");
    let (ctx, mut tab) = open(&dir);
    let before = tip(&dir, "main");
    let item = DragItem::Commit(commit_item(&tab, &c));
    let target = DropTarget::Commit(commit_item(&tab, &a));
    let offers = offered(&tab, item, &target);
    let short = |h: &str| crate::repo::short(h).to_owned();
    let move_below = format!("Move {} below {} “a”", short(&c), short(&a));
    assert_eq!(
        labels(&offers),
        [
            format!("Move {} above {} “a”", short(&c), short(&a)),
            move_below.clone(),
            format!("Squash {} into {} “a”", short(&c), short(&a)),
            format!("Fixup {} into {} “a”", short(&c), short(&a)),
        ]
    );
    choose(&mut tab, &ctx, &offers, &move_below);
    let moved = subjects(&dir);
    let files = git_ok(&dir, &["ls-files"]);
    run(&mut tab, &ctx, Action::Undo);
    let undone = tip(&dir, "main");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(moved, ["b", "a", "c", "base"]);
    assert_eq!(files.lines().count(), 4, "{files}");
    assert_eq!(undone, before);
}

#[test]
fn a_commit_dropped_on_another_of_the_branch_is_squashed_into_it() {
    let (dir, [a, _b, c]) = three("squash");
    let (ctx, mut tab) = open(&dir);
    let item = DragItem::Commit(commit_item(&tab, &c));
    let target = DropTarget::Commit(commit_item(&tab, &a));
    let offers = offered(&tab, item, &target);
    let short = |h: &str| crate::repo::short(h).to_owned();
    choose(
        &mut tab,
        &ctx,
        &offers,
        &format!("Squash {} into {} “a”", short(&c), short(&a)),
    );
    let squashed = subjects(&dir);
    let message = git_ok(&dir, &["log", "-1", "--format=%B", "main~1"]);
    let files = git_ok(&dir, &["show", "--format=", "--name-only", "main~1"]);
    // Then b fixed up into the squashed commit, which keeps only its message.
    let squashed_a = git::resolve(&dir, "main~1").unwrap_or_default();
    let item = DragItem::Commit(commit_item(&tab, &tip(&dir, "main")));
    let target = DropTarget::Commit(commit_item(&tab, &squashed_a));
    let offers = offered(&tab, item, &target);
    let fixup = format!(
        "Fixup {} into {} “a”",
        short(&tip(&dir, "main")),
        short(&squashed_a)
    );
    // A rewrite's receipt arrives separately from its operation and reload. Deliver one
    // from the squash while the following fixup is still preparing in the background.
    let receipt = format!(
        "{} (late receipt)",
        tab.status.as_deref().map_or("", |(message, _)| message)
    );
    assert!(
        tab.tx
            .send(crate::repo::Msg::Then(Box::new(move |tab, _| {
                tab.status = Some(crate::repo::Notice::plain((receipt, false)));
            })))
            .is_ok()
    );
    choose(&mut tab, &ctx, &offers, &fixup);
    let fixed = subjects(&dir);
    let fixed_message = git_ok(&dir, &["log", "-1", "--format=%B", "main"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(squashed, ["b", "a", "base"]);
    assert!(!message.contains('#'), "{message}");
    assert_eq!(
        message.split_whitespace().collect::<Vec<_>>(),
        ["a", "c"],
        "both messages kept"
    );
    assert_eq!(files.lines().collect::<Vec<_>>(), ["a.txt", "c.txt"]);
    assert_eq!(fixed, ["a", "base"], "fixup status: {:?}", tab.status);
    assert_eq!(
        fixed_message.split_whitespace().collect::<Vec<_>>(),
        ["a", "c"]
    );
}

/// A rewrite chosen from the menu after the branch got a commit (in the terminal, while the
/// menu was open) reads the branch as it is then: the new commit is replayed, not lost.
#[test]
fn a_reorder_keeps_a_commit_made_since_the_drop() {
    let (dir, [a, _b, c]) = three("reorder-moved");
    let (ctx, mut tab) = open(&dir);
    let item = DragItem::Commit(commit_item(&tab, &c));
    let target = DropTarget::Commit(commit_item(&tab, &a));
    let offers = offered(&tab, item, &target);
    commit(&dir, "d.txt", "d");
    let short = |h: &str| crate::repo::short(h).to_owned();
    choose(
        &mut tab,
        &ctx,
        &offers,
        &format!("Move {} below {} “a”", short(&c), short(&a)),
    );
    let after = subjects(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(after, ["d", "b", "a", "c", "base"]);
}
