//! What the palette offers for a repository: no entry per branch and verb, the selection's
//! entries, the second step's choices.

use std::sync::Arc;

use super::{PaletteItem, PaletteRun, Pick};
use crate::git::{Area, Branch, Commit, StatusFile};
use crate::repo::{Action, DiffKey, DiffView, RepoTab, WIP_HASH};

fn tab() -> RepoTab {
    let ctx = egui::Context::default();
    RepoTab::new_deferred("/nonexistent/palette-test".into(), &ctx)
}

fn branch(name: &str, head: bool) -> Branch {
    Branch {
        name: name.to_owned(),
        target: "1".repeat(40),
        is_head: head,
        upstream: String::new(),
        track: String::new(),
    }
}

/// A tab on `main` with `n` other local branches, and one remote branch.
fn tab_with_branches(n: usize) -> RepoTab {
    let mut tab = tab();
    tab.refs.head = "a".repeat(40);
    tab.refs.current = Some("main".to_owned());
    tab.refs.local.push(branch("main", true));
    for k in 0..n {
        tab.refs.local.push(branch(&format!("feature/{k}"), false));
    }
    tab.refs
        .remotes
        .push(("origin".to_owned(), vec![branch("origin/main", false)]));
    tab
}

fn commit(hash: &str, parents: usize) -> Arc<Commit> {
    Arc::new(Commit {
        hash: hash.to_owned(),
        parents: vec!["0".repeat(40); parents],
        summary: "the summary".to_owned(),
        ..Commit::default()
    })
}

fn labels(items: &[PaletteItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

#[test]
fn branches_do_not_flood_the_first_step() {
    let few = tab_with_branches(1).palette_items().len();
    let many = tab_with_branches(3000).palette_items().len();
    assert_eq!(few, many);
    let tab = tab_with_branches(3000);
    let items = tab.palette_items();
    let got = labels(&items);
    for verb in [
        "Check out a branch…",
        "Merge into main…",
        "Rebase main onto…",
        "Interactive rebase main onto…",
        "Changes a branch adds to main…",
        "Compare main with…",
    ] {
        assert!(got.contains(&verb), "{verb} missing from {got:?}");
    }
    assert!(!got.iter().any(|l| l.contains("feature/")));
}

#[test]
fn the_second_step_lists_every_other_branch() {
    let tab = tab_with_branches(3000);
    let merge = tab.palette_choices(&Pick::Merge);
    // Every local branch but main, and origin/main.
    assert_eq!(merge.len(), 3001);
    assert!(!labels(&merge).contains(&"main"));
    let remote = merge.iter().find(|i| i.label == "origin/main");
    assert_eq!(remote.map(|i| i.hint.as_str()), Some("origin"));
    assert!(merge.iter().all(|i| i.key.starts_with("merge:")));
    assert!(matches!(
        &merge[0].run,
        PaletteRun::Action(Action::UndoableOps { label, ops, .. })
            if label == "Merge feature/0 into main"
                && ops[0].last().map(String::as_str) == Some("refs/heads/feature/0")
    ));
    let checkout = tab.palette_choices(&Pick::Checkout);
    assert!(matches!(
        &checkout.last().map(|i| &i.run),
        Some(PaletteRun::Action(Action::CheckoutRemote(name))) if name == "origin/main"
    ));
}

#[test]
fn with_only_one_branch_there_is_nothing_to_pick() {
    let mut tab = tab_with_branches(0);
    tab.refs.remotes.clear();
    let items = tab.palette_items();
    assert!(!items.iter().any(|i| matches!(i.run, PaletteRun::Pick(_))));
    assert!(tab.palette_choices(&Pick::Merge).is_empty());
}

#[test]
fn detached_head_offers_checkout_but_nothing_into_the_branch() {
    let mut tab = tab_with_branches(2);
    tab.refs.current = None;
    tab.refs.local[0].is_head = false;
    let items = tab.palette_items();
    let got = labels(&items);
    assert!(got.contains(&"Check out a branch…"));
    assert!(!got.iter().any(|l| l.starts_with("Merge into")));
    assert!(tab.palette_choices(&Pick::Merge).is_empty());
    assert_eq!(tab.palette_choices(&Pick::Checkout).len(), 4);
}

#[test]
fn a_selected_commit_puts_its_actions_first() {
    let mut tab = tab_with_branches(1);
    let hash = "a301da9c".to_owned() + &"0".repeat(32);
    tab.commits = vec![commit(&tab.refs.head.clone(), 1), commit(&hash, 1)];
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    tab.selected = Some(1);
    let items = tab.palette_items();
    let got = labels(&items);
    assert_eq!(
        got[..4],
        [
            "Cherry-pick a301da9c",
            "Revert a301da9c",
            "Reset main to here…",
            "Interactive rebase main onto a301da9c…"
        ]
    );
    assert!(items.iter().take_while(|i| i.selection).count() >= 6);
    let reset = tab.palette_choices(&Pick::Reset(hash.clone()));
    assert_eq!(reset.len(), 3);
    assert!(reset[2].label.starts_with("Hard"));
    assert!(matches!(
        &reset[0].run,
        PaletteRun::Action(Action::Dialog(_))
    ));
    // A commit gone since (the history reloaded) has nothing to reset to.
    assert!(tab.palette_choices(&Pick::Reset("f".repeat(40))).is_empty());

    // HEAD itself: nothing to cherry-pick or reset to; reverting it is still possible.
    tab.selected = Some(0);
    let items = tab.palette_items();
    let got = labels(&items);
    assert!(!got.iter().any(|l| l.starts_with("Cherry-pick")));
    assert!(!got.iter().any(|l| l.starts_with("Reset")));
    assert!(got[0].starts_with("Revert"));

    // While an operation runs, nothing rewrites the branch.
    tab.selected = Some(1);
    tab.busy_op = Some("Fetch".to_owned());
    let items = tab.palette_items();
    let got = labels(&items);
    assert!(
        !got.iter()
            .any(|l| l.starts_with("Cherry-pick") || l.starts_with("Revert"))
    );
    assert!(got[0].starts_with("Create branch at"));
}

#[test]
fn the_uncommitted_changes_row_and_merges_offer_less() {
    let mut tab = tab_with_branches(1);
    tab.commits = vec![commit(WIP_HASH, 1), commit(&"b".repeat(40), 2)];
    tab.selected = Some(0);
    assert!(!tab.palette_items().iter().any(|i| i.selection));
    tab.selected = Some(1);
    let items = tab.palette_items();
    assert!(!labels(&items).iter().any(|l| l.starts_with("Create patch")));
    // A selection past the end (the history shrank) is no selection.
    tab.selected = Some(7);
    assert!(!tab.palette_items().iter().any(|i| i.selection));
}

fn work(path: &str, code: char, area: Area) -> DiffKey {
    DiffKey::Work(StatusFile {
        path: path.to_owned(),
        old_path: None,
        code,
        area,
    })
}

fn file_labels(tab: &mut RepoTab, key: DiffKey) -> Vec<String> {
    tab.diff = Some(DiffView::new(key));
    tab.palette_items()
        .into_iter()
        .filter(|i| i.selection)
        .map(|i| i.label)
        .collect()
}

#[test]
fn the_open_file_has_its_actions() {
    let mut tab = tab_with_branches(1);
    let path = "dir with spaces/ünï 文件.txt";
    let got = file_labels(&mut tab, work(path, 'M', Area::Unstaged));
    assert_eq!(got[0], format!("Stage {path}"));
    assert_eq!(got[1], format!("Discard changes to {path}…"));
    assert!(got.contains(&format!("Blame {path}")));
    assert!(got.contains(&format!("Open {path} in editor")));

    let got = file_labels(&mut tab, work(path, '?', Area::Untracked));
    assert_eq!(got[1], format!("Delete {path}…"));

    let got = file_labels(&mut tab, work(path, 'M', Area::Staged));
    assert_eq!(got[0], format!("Unstage {path}"));

    // Deleted: nothing to view or open; conflicted: resolved in the conflict editor.
    let got = file_labels(&mut tab, work(path, 'D', Area::Unstaged));
    assert_eq!(got.len(), 2);
    let got = file_labels(&mut tab, work(path, 'U', Area::Conflicted));
    assert!(!got.iter().any(|l| l.starts_with("Stage")));

    // In a commit: viewed as it is there, not opened from the working tree.
    let key = DiffKey::Commit {
        hash: "c".repeat(40),
        path: path.to_owned(),
    };
    tab.diff = Some(DiffView::new(key));
    let items = tab.palette_items();
    let history = items
        .iter()
        .find(|i| i.label == format!("File history of {path}"));
    assert!(matches!(
        history.map(|i| &i.run),
        Some(PaletteRun::Action(Action::FileView { rev: Some(rev), .. })) if *rev == "c".repeat(40)
    ));
    assert!(
        !items
            .iter()
            .any(|i| i.label.starts_with("Open ") && i.selection)
    );
}

#[test]
fn the_generation_changes_with_the_selection() {
    let mut tab = tab_with_branches(1);
    let before = tab.palette_generation();
    tab.selected = Some(0);
    assert_ne!(before, tab.palette_generation());
    let selected = tab.palette_generation();
    tab.diff = Some(DiffView::new(work("a", 'M', Area::Unstaged)));
    assert_ne!(selected, tab.palette_generation());
    let opened = tab.palette_generation();
    tab.busy_op = Some("Push".to_owned());
    assert_ne!(opened, tab.palette_generation());
}
