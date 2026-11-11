// Included in the parent test module to preserve qualified test names.

#[test]
fn branches_do_not_flood_the_first_step() {
    let few = tab_with_branches(1)
        .palette_items(&egui::Context::default())
        .len();
    let many = tab_with_branches(3000)
        .palette_items(&egui::Context::default())
        .len();
    assert_eq!(few, many);
    let tab = tab_with_branches(3000);
    let items = tab.palette_items(&egui::Context::default());
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
fn operation_looking_branch_names_and_remote_hints_remain_opaque_in_both_locales()
-> Result<(), String> {
    let mut tab = tab_with_branches(0);
    let names = ["Fetch", "Undo Fetch", "Check out topic", "001234"];
    tab.refs.local.extend(names.map(|name| branch(name, false)));
    tab.refs.remotes = vec![("Fetch".into(), vec![branch("Fetch/topic", false)])];
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        for (pick, key) in [(Pick::Checkout, "checkout"), (Pick::Merge, "merge")] {
            let choices = tab.palette_choices(&pick);
            for name in names {
                let item = choices.iter().find(|i| i.key == format!("{key}:{name}"));
                assert_eq!(item.map(|i| i.label.as_str()), Some(name));
                let item = item.ok_or("actual branch choice missing")?;
                match (&pick, &item.run) {
                    (Pick::Checkout, PaletteRun::Action(Action::Checkout(raw))) => {
                        assert_eq!(raw, name);
                    }
                    (Pick::Merge, PaletteRun::Action(Action::UndoableOps { label, ops, .. })) => {
                        assert_eq!(label, &format!("Merge {name} into main"));
                        assert_eq!(ops[0].last(), Some(&format!("refs/heads/{name}")));
                    }
                    _ => return Err("branch dispatch changed".into()),
                }
            }
            let remote = choices.iter().find(|i| i.label == "Fetch/topic");
            assert_eq!(remote.map(|i| i.hint.as_str()), Some("Fetch"));
        }
    }
    Ok(())
}

#[test]
fn with_only_one_branch_there_is_nothing_to_pick() {
    let mut tab = tab_with_branches(0);
    tab.refs.remotes.clear();
    let items = tab.palette_items(&egui::Context::default());
    assert!(!items.iter().any(|i| matches!(i.run, PaletteRun::Pick(_))));
    assert!(tab.palette_choices(&Pick::Merge).is_empty());
}

#[test]
fn detached_head_offers_checkout_but_nothing_into_the_branch() {
    let mut tab = tab_with_branches(2);
    tab.refs.current = None;
    tab.refs.local[0].is_head = false;
    let items = tab.palette_items(&egui::Context::default());
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
    tab.history = tab.commits.clone();
    tab.history_index = tab.index.clone();
    tab.selected = Some(1);
    let items = tab.palette_items(&egui::Context::default());
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
    assert_eq!(reset.len(), 4);
    for (item, mode) in reset.iter().zip(["Soft", "Mixed", "Keep", "Hard"]) {
        assert!(item.label.starts_with(mode));
    }
    assert!(matches!(
        &reset[0].run,
        PaletteRun::Action(Action::Dialog(_))
    ));
    // A commit gone since (the history reloaded) has nothing to reset to.
    assert!(tab.palette_choices(&Pick::Reset("f".repeat(40))).is_empty());

    // HEAD itself: nothing to cherry-pick or reset to; reverting it is still possible.
    tab.selected = Some(0);
    let items = tab.palette_items(&egui::Context::default());
    let got = labels(&items);
    assert!(!got.iter().any(|l| l.starts_with("Cherry-pick")));
    assert!(!got.iter().any(|l| l.starts_with("Reset")));
    assert!(got[0].starts_with("Revert"));

    // While an operation runs, nothing rewrites the branch.
    tab.selected = Some(1);
    tab.busy_op = Some("Fetch".to_owned());
    let items = tab.palette_items(&egui::Context::default());
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
    assert!(
        !tab.palette_items(&egui::Context::default())
            .iter()
            .any(|i| i.selection)
    );
    tab.history = tab.commits.clone();
    tab.history_index = tab.index.clone();
    tab.selected = Some(1);
    let items = tab.palette_items(&egui::Context::default());
    assert!(!labels(&items).iter().any(|l| l.starts_with("Create patch")));
    // A selection past the end (the history shrank) is no selection.
    tab.selected = Some(7);
    assert!(
        !tab.palette_items(&egui::Context::default())
            .iter()
            .any(|i| i.selection)
    );
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
    let items = tab.palette_items(&egui::Context::default());
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
