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
    tab.palette_items(&egui::Context::default())
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

/// Pure supplied-state regression for REG-R2-1: actual entries reach the production
/// Action::Key adapter and clipboard output, with no OS clipboard, Git or background read.
#[test]
fn registry_palette_selected_copy_dispatches_and_unowned_contexts_are_absent() -> Result<(), String>
{
    use crate::keymap::Command;
    let ctx = egui::Context::default();
    let mut tab = tab_with_branches(0);
    let hash = tab.refs.head.clone();
    tab.commits = vec![Arc::new(Commit {
        hash: hash.clone(),
        summary: "selected ü message".into(),
        refs: vec!["main".into()],
        ..Default::default()
    })];
    tab.selected = Some(0);
    for (command, expected) in [
        (Command::GraphCopyHash, hash.as_str()),
        (Command::GraphCopyMessage, "selected ü message"),
        (Command::GraphCopyBranch, "main"),
    ] {
        let item = tab
            .palette_items(&ctx)
            .into_iter()
            .find(|i| matches!(&i.run, PaletteRun::Action(Action::Key(c)) if *c == command))
            .ok_or("missing applicable selected-commit command")?;
        let PaletteRun::Action(action) = item.run else {
            return Err("not an action".into());
        };
        let mut action = Some(action);
        let out = ctx.run_ui(egui::RawInput::default(), |_| {
            if let Some(action) = action.take() {
                tab.apply(action, &ctx);
            }
        });
        assert_eq!(
            out.platform_output
                .commands
                .iter()
                .filter(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == expected))
                .count(),
            1
        );
        out.drop_without_applying_deltas();
    }
    // No generic palette entries for widget-owned find, selection, file-row or rebase
    // commands. Their keyboard handlers retain the actual view/focus and target ownership.
    for item in tab.palette_items(&ctx) {
        if let PaletteRun::Action(Action::Key(c)) = item.run {
            assert!(
                !matches!(tab.key_action(c), Ok(None)),
                "inert palette entry: {c:?}"
            );
            assert!(!matches!(
                c.scope(),
                crate::keymap::Scope::Find
                    | crate::keymap::Scope::Selection
                    | crate::keymap::Scope::Rebase
            ));
        }
    }
    tab.selected = None;
    assert!(tab.palette_items(&ctx).iter().all(|i| !matches!(
        &i.run,
        PaletteRun::Action(Action::Key(
            Command::GraphCopyHash | Command::GraphCopyMessage | Command::GraphCopyBranch
        ))
    )));
    assert!(matches!(
        tab.key_action(Command::Reload),
        Ok(Some(Action::Reload))
    ));
    tab.apply(Action::Key(Command::RebasePick), &ctx);
    assert!(
        tab.status
            .as_deref()
            .is_some_and(|(why, bad)| *bad && why.contains("owning view"))
    );
    Ok(())
}

#[test]
fn registry_palette_effective_undo_redo_search_keys_are_unique_under_override_unbind_reset() {
    use crate::keymap::{self, Command, Stroke};
    let ctx = egui::Context::default();
    let tab = tab();
    let mut settings = crate::settings::Settings::default();
    for mode in 0..3 {
        for (command, key) in [
            (Command::Undo, egui::Key::F8),
            (Command::Redo, egui::Key::F9),
            (Command::Search, egui::Key::F10),
        ] {
            match mode {
                0 => {
                    settings.key_bindings.insert(
                        command.id().into(),
                        vec![Stroke::new(key, egui::Modifiers::NONE)],
                    );
                }
                1 => {
                    settings.key_bindings.insert(command.id().into(), vec![]);
                }
                _ => {
                    settings.key_bindings.remove(command.id());
                }
            }
        }
        settings.put_for_test(&ctx);
        let items = tab.palette_items(&ctx);
        for command in [Command::Undo, Command::Redo, Command::Search] {
            let matches: Vec<_> = items
                .iter()
                .filter(
                    |i| matches!(&i.run, PaletteRun::Action(a) if a.key_command() == Some(command)),
                )
                .collect();
            assert_eq!(matches.len(), 1, "mode {mode}: {command:?}");
            let label = keymap::label(&ctx, command);
            assert_eq!(matches[0].shortcut, (!label.is_empty()).then_some(label));
            assert!(matches!(&matches[0].run, PaletteRun::Action(Action::Key(c)) if *c == command));
        }
    }
}

#[test]
fn registry_palette_search_dispatches_the_same_visible_target_as_the_key() -> Result<(), String> {
    use crate::keymap::Command;
    let ctx = egui::Context::default();
    let mut tab = tab();
    for lines in [false, true] {
        tab.search.open = false;
        tab.search.focus = false;
        tab.find.close();
        tab.diff = lines.then(|| {
            DiffView::new(DiffKey::Commit {
                hash: "recorded".into(),
                path: "file".into(),
            })
        });
        let item = tab
            .palette_items(&ctx)
            .into_iter()
            .find(|i| matches!(&i.run, PaletteRun::Action(Action::Key(Command::Search))))
            .ok_or("missing Search entry")?;
        let PaletteRun::Action(action) = item.run else {
            return Err("not an action".into());
        };
        tab.apply(action, &ctx);
        assert_eq!(tab.find.open, lines);
        assert_eq!(tab.find.focus, lines);
        assert_eq!(tab.search.open, !lines);
        assert_eq!(tab.search.focus, !lines);
    }
    Ok(())
}

#[test]
fn area_d_hidden_optional_tools_stay_in_palette_with_selected_commit_scope() {
    let mut tab = tab_with_branches(1);
    let head = "a".repeat(40);
    let selected = "b".repeat(40);
    let root = "c".repeat(40);
    tab.commits = vec![
        std::sync::Arc::new(Commit {
            hash: head.clone(),
            parents: vec![selected.clone()],
            ..Default::default()
        }),
        std::sync::Arc::new(Commit {
            hash: selected.clone(),
            parents: vec![root.clone()],
            ..Default::default()
        }),
        std::sync::Arc::new(Commit {
            hash: root,
            ..Default::default()
        }),
    ];
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
    for key in [
        "commit:split",
        "commit:move",
        "commit:bisect",
        "commit:parallel-bisect",
        "commit:test",
        "commit:release",
        "pick:rehearse",
    ] {
        assert!(
            items.iter().any(|i| i.key == key),
            "{key} missing with default (hidden) menu options"
        );
    }
    assert!(items.iter().any(
        |i| matches!(&i.run, PaletteRun::Action(Action::OpenSplit(hash)) if hash == &selected)
    ));
    assert!(items.iter().any(
        |i| matches!(&i.run, PaletteRun::Action(Action::OpenMove(hash)) if hash == &selected)
    ));
    tab.refs.current = None;
    assert!(
        !tab.palette_items(&egui::Context::default())
            .iter()
            .any(|i| matches!(
                &i.run,
                PaletteRun::Action(Action::OpenSplit(_) | Action::OpenMove(_))
            ))
    );
}
