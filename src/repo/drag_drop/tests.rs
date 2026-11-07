//! Drops on real repositories, through the tab: what each offers, what choosing it does to
//! the repository, and undoing it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{DragBranch, DragCommit, DragItem, DropTarget, Offer};
use crate::git;
use crate::repo::{Action, RepoTab};

/// A repository on `main` with the commit `base`, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-dnd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        git_ok(&dir, args);
    }
    commit(&dir, "base.txt", "base");
    dir
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
    out.unwrap_or_default()
}

/// Commits the file `file` (its contents the message) and returns the commit.
fn commit(dir: &Path, file: &str, message: &str) -> String {
    let _ = std::fs::write(dir.join(file), format!("{message}\n"));
    git_ok(dir, &["add", file]);
    git_ok(dir, &["commit", "-q", "-m", message]);
    git::head(dir)
}

fn tip(dir: &Path, branch: &str) -> String {
    git::resolve(dir, &format!("refs/heads/{branch}")).unwrap_or_default()
}

/// Subjects from HEAD down its first parents.
fn subjects(dir: &Path) -> Vec<String> {
    git_ok(dir, &["log", "--first-parent", "--format=%s"])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Polls until the requested state arrives. Slow Git commands must not leave
/// the test inspecting a rewrite that is still running.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let start = Instant::now();
    loop {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "drop action did not finish: loading={}, busy={:?}, status={:?}",
            tab.loading,
            tab.busy_op,
            tab.status
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Polls until nothing runs and the reload after it is in.
fn settle(tab: &mut RepoTab, ctx: &egui::Context) {
    wait(tab, ctx, |t| {
        t.busy_op.is_none() && !t.loading && !t.commits.is_empty()
    });
}

fn open(dir: &Path) -> (egui::Context, RepoTab) {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.to_owned(), &ctx);
    settle(&mut tab, &ctx);
    (ctx, tab)
}

/// Runs `action` as a click on it would, until it and the reload after it are done.
fn run(tab: &mut RepoTab, ctx: &egui::Context, action: Action) {
    let status = tab.status.clone();
    let completed = tab.activity.last().map(|entry| entry.id);
    tab.apply(action, ctx);
    // Some first read what they rewrite in the background.
    // An earlier operation's receipt can change its successful status during that read.
    // Use the activity id to also catch an operation that finished within a single poll.
    wait(tab, ctx, |t| {
        t.busy_op.is_some()
            || t.activity.last().map(|entry| entry.id) != completed
            || (t.status != status && t.status.as_ref().is_some_and(|(_, failed)| *failed))
    });
    settle(tab, ctx);
}

/// What dropping `item` on `target` offers, by label.
fn offered(tab: &RepoTab, item: DragItem, target: &DropTarget) -> Vec<Offer> {
    tab.drop_offers(&tab.payload(item), target)
}

fn labels(offers: &[Offer]) -> Vec<String> {
    offers.iter().map(Offer::label).collect()
}

/// Chooses the offer `label` of `offers`.
fn choose(tab: &mut RepoTab, ctx: &egui::Context, offers: &[Offer], label: &str) {
    let offer = offers.iter().find(|o| o.label() == label);
    assert!(offer.is_some(), "{label} not in {:?}", labels(offers));
    if let Some(offer) = offer {
        run(tab, ctx, offer.action());
    }
}

fn branch(dir: &Path, name: &str) -> DragBranch {
    DragBranch {
        name: name.to_owned(),
        remote: false,
        tip: tip(dir, name),
    }
}

fn commit_item(tab: &RepoTab, hash: &str) -> DragCommit {
    let c = tab.index.get(hash).and_then(|&i| tab.commits.get(i));
    DragCommit {
        hash: hash.to_owned(),
        summary: c.map(|c| c.summary.clone()).unwrap_or_default(),
        merge: c.is_some_and(|c| c.parents.len() > 1),
    }
}

/// `main` and `feature` diverged from `base`: one commit each.
fn diverged(name: &str) -> PathBuf {
    let dir = repo(name);
    git_ok(&dir, &["branch", "feature"]);
    commit(&dir, "main.txt", "on main");
    git_ok(&dir, &["switch", "-q", "feature"]);
    commit(&dir, "feature.txt", "on feature");
    git_ok(&dir, &["switch", "-q", "main"]);
    dir
}

#[test]
fn dropping_a_branch_on_the_checked_out_one_merges_it_and_undo_takes_it_back() {
    let dir = diverged("merge");
    let (ctx, mut tab) = open(&dir);
    let before = tip(&dir, "main");
    let feature = tip(&dir, "feature");
    let target = DropTarget::Branch(branch(&dir, "main"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "feature")), &target);
    assert_eq!(
        labels(&offers),
        [
            "Merge feature into main",
            "Rebase main onto feature",
            "Interactive rebase main onto feature…",
        ]
    );
    choose(&mut tab, &ctx, &offers, "Merge feature into main");
    let parents = git_ok(&dir, &["log", "-1", "--format=%P"]);
    run(&mut tab, &ctx, Action::Undo);
    let undone = tip(&dir, "main");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        parents.split_whitespace().collect::<Vec<_>>(),
        [before.as_str(), feature.as_str()]
    );
    assert_eq!(undone, before);
}

#[test]
fn dropping_a_branch_on_the_checked_out_one_rebases_it() {
    let dir = diverged("rebase");
    let (ctx, mut tab) = open(&dir);
    let feature = tip(&dir, "feature");
    let target = DropTarget::Branch(branch(&dir, "main"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "feature")), &target);
    choose(&mut tab, &ctx, &offers, "Rebase main onto feature");
    let rebased = subjects(&dir);
    let parent = git_ok(&dir, &["rev-parse", "main^"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(rebased, ["on main", "on feature", "base"]);
    assert_eq!(parent.trim(), feature);
}

#[test]
fn the_interactive_rebase_opens_its_editor_onto_the_branch() {
    let dir = diverged("interactive");
    let (ctx, mut tab) = open(&dir);
    let target = DropTarget::Branch(branch(&dir, "main"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "feature")), &target);
    let label = "Interactive rebase main onto feature…";
    let offer = offers.iter().find(|o| o.label() == label).cloned();
    tab.apply(offer.map(|o| o.action()).unwrap_or(Action::Reload), &ctx);
    wait(&mut tab, &ctx, |t| t.rebase.is_some());
    let listed: Vec<String> = tab
        .rebase
        .iter()
        .flat_map(|r| r.entries.iter().map(|e| e.commit.summary.clone()))
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(listed, ["on main"]);
}

#[test]
fn a_branch_behind_is_only_fast_forwarded() {
    let dir = repo("ff-current");
    git_ok(&dir, &["switch", "-q", "-c", "feature"]);
    commit(&dir, "f.txt", "ahead");
    git_ok(&dir, &["switch", "-q", "main"]);
    let (ctx, mut tab) = open(&dir);
    let before = tip(&dir, "main");
    let target = DropTarget::Branch(branch(&dir, "main"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "feature")), &target);
    assert_eq!(labels(&offers), ["Fast-forward main to feature"]);
    choose(&mut tab, &ctx, &offers, "Fast-forward main to feature");
    let moved = tip(&dir, "main");
    run(&mut tab, &ctx, Action::Undo);
    let undone = tip(&dir, "main");
    let feature = tip(&dir, "feature");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(moved, feature);
    assert_eq!(undone, before);
}

#[test]
fn a_branch_not_checked_out_is_fast_forwarded_without_checking_it_out() {
    let dir = repo("ff-other");
    git_ok(&dir, &["branch", "topic"]);
    commit(&dir, "m.txt", "ahead on main");
    let (ctx, mut tab) = open(&dir);
    let before = tip(&dir, "topic");
    let main = tip(&dir, "main");
    let target = DropTarget::Branch(branch(&dir, "topic"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "main")), &target);
    assert_eq!(
        labels(&offers),
        [
            "Interactive rebase main onto topic…",
            "Fast-forward topic to main"
        ]
    );
    choose(&mut tab, &ctx, &offers, "Fast-forward topic to main");
    let moved = tip(&dir, "topic");
    let still_on = git_ok(&dir, &["symbolic-ref", "--short", "HEAD"]);
    run(&mut tab, &ctx, Action::Undo);
    let undone = tip(&dir, "topic");
    run(&mut tab, &ctx, Action::Redo);
    let redone = tip(&dir, "topic");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(moved, main);
    assert_eq!(still_on.trim(), "main");
    assert_eq!(undone, before);
    assert_eq!(redone, main);
}

/// A drop's fast-forward refuses once the branch it moves has moved since (a commit made in
/// the terminal while the menu was open): it would not be a fast-forward of what was shown.
#[test]
fn a_fast_forward_refuses_once_its_branch_moved() {
    let dir = repo("ff-moved");
    git_ok(&dir, &["branch", "topic"]);
    commit(&dir, "m.txt", "ahead on main");
    let (ctx, mut tab) = open(&dir);
    let target = DropTarget::Branch(branch(&dir, "topic"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "main")), &target);
    // Meanwhile, elsewhere: topic gets a commit of its own.
    git_ok(&dir, &["switch", "-q", "topic"]);
    let own = commit(&dir, "t.txt", "own work");
    git_ok(&dir, &["switch", "-q", "main"]);
    choose(&mut tab, &ctx, &offers, "Fast-forward topic to main");
    let after = tip(&dir, "topic");
    let failed = tab.status.as_ref().is_some_and(|(_, err)| *err);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(after, own);
    assert!(failed);
}

/// Nor does a merge go into another branch than the one it was dropped on, checked out since.
#[test]
fn a_merge_refuses_once_another_branch_is_checked_out() {
    let dir = diverged("merge-moved");
    let (ctx, mut tab) = open(&dir);
    let target = DropTarget::Branch(branch(&dir, "main"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "feature")), &target);
    git_ok(&dir, &["switch", "-q", "-c", "other", "main"]);
    choose(&mut tab, &ctx, &offers, "Merge feature into main");
    let other = git_ok(&dir, &["log", "-1", "--format=%P", "other"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(other.split_whitespace().count(), 1, "other got the merge");
}

#[test]
fn a_commit_dropped_on_the_checked_out_branch_is_cherry_picked() {
    let dir = diverged("cherry-pick");
    let (ctx, mut tab) = open(&dir);
    let before = tip(&dir, "main");
    let picked = tip(&dir, "feature");
    let item = DragItem::Commit(commit_item(&tab, &picked));
    let target = DropTarget::Branch(branch(&dir, "main"));
    let offers = offered(&tab, item.clone(), &target);
    let label = format!("Cherry-pick {} onto main", crate::repo::short(&picked));
    assert_eq!(labels(&offers), std::slice::from_ref(&label));
    // Onto a branch that isn't checked out, nothing.
    let elsewhere = DropTarget::Branch(branch(&dir, "feature"));
    assert!(offered(&tab, item, &elsewhere).is_empty());
    choose(&mut tab, &ctx, &offers, &label);
    let after = subjects(&dir);
    run(&mut tab, &ctx, Action::Undo);
    let undone = tip(&dir, "main");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(after, ["on feature", "on main", "base"]);
    assert_eq!(undone, before);
}

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
        tab.status.as_ref().map_or("", |(message, _)| message)
    );
    assert!(
        tab.tx
            .send(crate::repo::Msg::Then(Box::new(move |tab, _| {
                tab.status = Some((receipt, false));
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

/// A fast-forward of another branch refuses once that branch is checked out (in the
/// terminal, while the menu was open): moving it under its checkout would leave the working
/// tree at the old commit, its changes staged as their reverse.
#[test]
fn a_fast_forward_refuses_once_its_branch_is_checked_out() {
    let dir = repo("ff-checked-out");
    git_ok(&dir, &["branch", "topic"]);
    commit(&dir, "m.txt", "ahead on main");
    let (ctx, mut tab) = open(&dir);
    let before = tip(&dir, "topic");
    let target = DropTarget::Branch(branch(&dir, "topic"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "main")), &target);
    git_ok(&dir, &["switch", "-q", "topic"]);
    choose(&mut tab, &ctx, &offers, "Fast-forward topic to main");
    let after = tip(&dir, "topic");
    let staged = git_ok(&dir, &["diff", "--cached", "--name-only"]);
    let failed = tab.status.as_ref().is_some_and(|(_, err)| *err);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(after, before);
    assert!(staged.is_empty(), "{staged}");
    assert!(failed);
}

/// Nor while it is being rebased in another worktree, where git lists it as detached: the
/// rebase could not finish onto a branch moved under it.
#[test]
fn a_fast_forward_refuses_a_branch_being_rebased_elsewhere() {
    let dir = repo("ff-rebasing");
    git_ok(&dir, &["branch", "topic"]);
    commit(&dir, "m.txt", "ahead on main");
    let (ctx, mut tab) = open(&dir);
    let before = tip(&dir, "topic");
    let target = DropTarget::Branch(branch(&dir, "topic"));
    let offers = offered(&tab, DragItem::Branch(branch(&dir, "main")), &target);
    let wt = dir.with_extension("wt");
    let _ = std::fs::remove_dir_all(&wt);
    let wt_path = wt.to_string_lossy().into_owned();
    git_ok(&dir, &["worktree", "add", "-q", &wt_path, "topic"]);
    // A rebase stopped at its first step, the branch detached meanwhile.
    git_ok(
        &wt,
        &[
            "-c",
            "sequence.editor=sed -i.bak s/^pick/edit/",
            "rebase",
            "-q",
            "-i",
            "--root",
        ],
    );
    choose(&mut tab, &ctx, &offers, "Fast-forward topic to main");
    let after = tip(&dir, "topic");
    let _ = git::run(Some(&wt), &["rebase", "--abort"]);
    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(after, before);
}

#[test]
fn filtered_ancestor_keeps_rewrite_offers_and_drop_replays_hidden_commit_then_undoes()
-> Result<(), String> {
    let dir = repo("filtered-ancestor");
    let older = commit(&dir, "older.txt", "older");
    git_ok(&dir, &["config", "user.name", "T3 Code"]);
    git_ok(
        &dir,
        &["config", "user.email", "t3code@users.noreply.github.com"],
    );
    let hidden = commit(&dir, "hidden.txt", "hidden checkpoint");
    git_ok(&dir, &["config", "user.name", "Human"]);
    git_ok(&dir, &["config", "user.email", "human@example.com"]);
    let head = commit(&dir, "head.txt", "head");
    let (ctx, mut tab) = open(&dir);
    assert!(!tab.index.contains_key(&hidden));
    assert!(tab.history_index.contains_key(&hidden));
    let offers = offered(
        &tab,
        DragItem::Commit(commit_item(&tab, &older)),
        &DropTarget::Commit(commit_item(&tab, &head)),
    );
    assert!(!offers.iter().any(|o| matches!(o, Offer::CherryPick { .. })));
    assert!(offers.iter().any(|o| matches!(o, Offer::Rewrite(_))));
    assert_eq!(tab.surgery_refusal(&older), None);
    run(
        &mut tab,
        &ctx,
        Action::QuickEdit {
            edit: crate::repo::quick_edit::Edit::Drop,
            hash: older,
        },
    );
    assert_eq!(subjects(&dir), ["head", "hidden checkpoint", "base"]);
    assert_eq!(
        std::fs::read_to_string(dir.join("hidden.txt")).map_err(|e| e.to_string())?,
        "hidden checkpoint\n"
    );
    run(&mut tab, &ctx, Action::Undo);
    assert_eq!(git::head(&dir), head);
    assert_eq!(
        subjects(&dir),
        ["head", "hidden checkpoint", "older", "base"]
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
